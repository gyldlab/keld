//! One retained, main-thread-owned application menu. Authentication and Guard
//! remain with Core; no native activation itself confers application authority.
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::RefCell;
use std::rc::Rc;

use keld_ipc::IpcError;
use objc2::MainThreadMarker;
use objc2::rc::{Retained, Weak};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSMenu,
    NSMenuItem, NSResponder, NSWindow,
};
use objc2_foundation::NSString;

use super::{
    MenuAction, MenuInstallReceipt, MenuKind, MenuRequestV1, MenuRole, validate_menu_request,
};

mod target;

use target::MenuTarget;

/// Borrowed access to the existing Host UI registry's close-request route.
/// The native window type remains owned by this module; Core can pass this
/// callback through without importing `AppKit` or interpreting native handles.
/// Acceptance requests the existing close/veto sequence, never physical close.
pub type MenuCloseRequest<'a> = dyn FnMut(&NSWindow) -> Result<(), IpcError> + 'a;

/// One actual native target invocation. These numbers are Host-local facts;
/// they are never accepted from an application IPC payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuActivation {
    /// Immutable native tree generation, minted only after successful install.
    pub generation: u64,
    /// Immutable preorder node position in that generation.
    pub command: u16,
    /// One-shot native invocation; prevents replay within the same generation.
    pub sequence: u64,
}

/// Read-only `AppKit` focus observations. They are never alternative target
/// selection or authorization: the currently key native window and the
/// registry's Close gate remain mandatory at final dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuFocusEvidence {
    /// Observations captured when `AppKit` invoked the menu target.
    pub at_click: MenuFocusSnapshot,
    /// Observations captured when Core began its final dispatch check.
    pub before_dispatch: MenuFocusSnapshot,
}

/// Three independent presence observations at one native sampling point.
/// These values neither retain a window nor select an action target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuFocusSnapshot {
    /// Whether the Host application was active.
    pub app_active: bool,
    /// Whether `AppKit` reported a key window.
    pub key_window_present: bool,
    /// Whether `AppKit` reported a main window.
    pub main_window_present: bool,
}

/// Observable result of one already-authorized native action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacMenuOutcome {
    /// An actual `AppKit` application or responder action was delivered.
    Applied,
    /// The existing Host close-request path accepted the request; this is
    /// neither a close-veto decision nor proof of physical window release.
    CloseRequested,
    /// No focused responder implemented the fixed native edit action.
    UnsupportedResponder,
    /// Frozen custom-click/checkbox action; Core reports COMPAT-053/GH556.
    UnsupportedClick,
    /// Held Quit action; Core reports COMPAT-054/GH563 without terminating.
    UnsupportedQuit,
}

struct PendingActivation {
    stamp: MenuActivation,
    action: MenuAction,
    focus_at_click: MenuFocusSnapshot,
    // Pending admission must not retain a View after its physical-release
    // receipt. Zeroing weak refs upgrade only for synchronous native dispatch.
    window: Option<Weak<NSWindow>>,
    responder: Option<Weak<NSResponder>>,
}

struct ActivationState {
    generation: u64,
    sequence: u64,
    active: bool,
    // One bounded slot. Reentrant/duplicate native invocations while an action
    // awaits Host admission cannot queue unbounded work or overwrite its focus.
    pending: Option<PendingActivation>,
    wake: Rc<dyn Fn()>,
}

fn invalid(detail: &'static str) -> IpcError {
    IpcError::Protocol { detail }
}

struct InstalledMenu {
    root: Retained<NSMenu>,
    items: Vec<Retained<NSMenuItem>>,
    // NSMenuItem.target is weak. Keep every exact target alive until its weak
    // item reference and action selector have both been cleared.
    targets: Vec<Retained<MenuTarget>>,
}

impl InstalledMenu {
    fn disable(&self) {
        for item in &self.items {
            item.setEnabled(false);
            item.setKeyEquivalent(&NSString::new());
        }
    }
}

impl Drop for InstalledMenu {
    fn drop(&mut self) {
        self.disable();
        for item in &self.items {
            MenuTarget::detach(item);
        }
        // The target vec drops only after all weak references are empty.
        self.targets.clear();
    }
}

/// Sole owner of a real `AppKit` application-menu tree and its native targets.
/// All retained handles and pending focus stay on the `AppKit` main thread.
///
/// Core must check the authenticated installing role and current menu Guard
/// grant immediately before install/dispatch, and call [`Self::cancel`] on
/// role retirement, revocation or terminal shutdown. A receipt or enabled bit
/// is not authority to skip those checks.
pub struct MacApplicationMenu {
    app: Retained<NSApplication>,
    state: Rc<RefCell<ActivationState>>,
    installed: Option<InstalledMenu>,
    next_generation: u64,
    main: MainThreadMarker,
}

impl std::fmt::Debug for MacApplicationMenu {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MacApplicationMenu")
            .field("generation", &self.state.borrow().generation)
            .field("installed", &self.installed.is_some())
            .finish_non_exhaustive()
    }
}

impl MacApplicationMenu {
    /// Create an inert owner on the main thread. `wake` only wakes the existing
    /// Host/UI service; it must not execute application or native action code.
    #[must_use]
    pub fn new(main: MainThreadMarker, wake: impl Fn() + 'static) -> Self {
        Self {
            app: NSApplication::sharedApplication(main),
            state: Rc::new(RefCell::new(ActivationState {
                generation: 0,
                sequence: 0,
                active: false,
                pending: None,
                wake: Rc::new(wake),
            })),
            installed: None,
            next_generation: 1,
            main,
        }
    }

    /// Materialize a validated replacement before swapping the actual menu.
    /// Labels are already resolved from the Host launch fact in the projection;
    /// only the sole typed role table supplies keys and modifier masks.
    ///
    /// # Errors
    /// Refuses invalid DTOs, generation exhaustion, failed target construction
    /// or an `AppKit` commit that cannot be observed. No partial tree is installed.
    pub fn install(&mut self, request: &MenuRequestV1) -> Result<MenuInstallReceipt, IpcError> {
        validate_menu_request(request)?;
        let generation = self.next_generation;
        let next_generation = generation
            .checked_add(1)
            .ok_or_else(|| invalid("native menu generation exhausted"))?;
        let replacement = self.materialize(request, generation)?;
        let prior_menu = self.app.mainMenu();
        // Reentrant old targets are already invalid before AppKit sees the
        // replacement. No external application callback runs in this cut.
        {
            let mut state = self.state.borrow_mut();
            state.active = false;
            state.pending = None;
        }
        self.app.setMainMenu(Some(&replacement.root));
        if !self.app.mainMenu().is_some_and(|actual| {
            std::ptr::eq::<NSMenu>(actual.as_ref(), replacement.root.as_ref())
        }) {
            self.app.setMainMenu(prior_menu.as_deref());
            // A failed native observation is fail-closed: old handles remain
            // visible but disabled until a deliberate successful installation.
            if let Some(installed) = &self.installed {
                installed.disable();
            }
            return Err(invalid("native application menu commit was not observed"));
        }
        self.installed = Some(replacement);
        self.next_generation = next_generation;
        {
            let mut state = self.state.borrow_mut();
            state.generation = generation;
            state.active = true;
        }
        Ok(MenuInstallReceipt { generation })
    }

    fn materialize(
        &self,
        request: &MenuRequestV1,
        generation: u64,
    ) -> Result<InstalledMenu, IpcError> {
        let root = NSMenu::new(self.main);
        root.setAutoenablesItems(false);
        let mut installed = InstalledMenu {
            root: root.clone(),
            items: Vec::with_capacity(request.nodes.len()),
            targets: Vec::with_capacity(request.nodes.len()),
        };
        let mut parents = vec![(root, true)];
        for (index, node) in request.nodes.iter().enumerate() {
            let depth = usize::from(node.depth);
            parents.truncate(depth + 1);
            let (parent, parent_enabled) = parents
                .get(depth)
                .ok_or_else(|| invalid("validated native menu parent is absent"))?;
            let enabled = *parent_enabled && node.enabled && node.kind != MenuKind::Separator;
            let item = if node.kind == MenuKind::Separator {
                NSMenuItem::separatorItem(self.main)
            } else {
                NSMenuItem::new(self.main)
            };
            item.setTitle(&NSString::from_str(&node.label));
            item.setEnabled(enabled);
            if let Some(checked) = node.checked {
                item.setState(if checked {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            }
            if let MenuAction::NativeRole(role) = node.action {
                item.setKeyEquivalent(&NSString::from_str(role.key()));
                item.setKeyEquivalentModifierMask(modifiers(role));
            }
            if node.action != MenuAction::Inert {
                let command = u16::try_from(index)
                    .map_err(|_| invalid("native menu command exceeds its bounded node index"))?;
                let target = MenuTarget::new(
                    self.main,
                    Rc::clone(&self.state),
                    generation,
                    command,
                    node.action,
                    enabled,
                )
                .ok_or_else(|| invalid("native application menu target could not initialize"))?;
                target.attach(&item);
                installed.targets.push(target);
            }
            parent.addItem(&item);
            if node.kind == MenuKind::Submenu {
                let submenu = NSMenu::new(self.main);
                submenu.setTitle(&NSString::from_str(&node.label));
                submenu.setAutoenablesItems(false);
                item.setSubmenu(Some(&submenu));
                parents.push((submenu, enabled));
            }
            installed.items.push(item);
        }
        Ok(installed)
    }

    /// Disable the entire actual tree, remove its key equivalents and discard
    /// pending focus before role loss, revocation, replacement or Host shutdown.
    /// `AppKit` updates cannot re-enable it because every menu disables autoenable.
    pub fn cancel(&mut self) {
        {
            let mut state = self.state.borrow_mut();
            state.active = false;
            state.pending = None;
        }
        if let Some(installed) = &self.installed {
            installed.disable();
        }
    }

    /// Read the one outstanding native invocation for Host authorization. The
    /// slot remains occupied until dispatch/discard; reading is not admission.
    #[must_use]
    pub fn poll_activation(&self) -> Option<MenuActivation> {
        self.state
            .borrow()
            .pending
            .as_ref()
            .map(|pending| pending.stamp)
    }

    /// Inspect the immutable action after authenticating the native stamp.
    ///
    /// # Errors
    /// Refuses cancelled, stale, mismatched and already-consumed invocations.
    pub fn action(&self, activation: MenuActivation) -> Result<MenuAction, IpcError> {
        let state = self.state.borrow();
        let pending = current_pending(&state, activation)?;
        Ok(pending.action)
    }

    /// Read-only `AppKit` menu identity observation for the Host activation
    /// diagnostic. It grants no dispatch authority: dispatch checks the
    /// actual installed menu again at the effect boundary.
    #[must_use]
    pub fn installed_tree_matches_main_menu(&self) -> bool {
        self.owns_actual_menu()
    }

    /// Observe `AppKit` presence only for the current native stamp, without
    /// promoting a main/unfocused/last window into menu action authority.
    #[must_use]
    pub fn focus_evidence(&self, activation: MenuActivation) -> Option<MenuFocusEvidence> {
        let state = self.state.borrow();
        let focus = current_pending(&state, activation).ok()?.focus_at_click;
        Some(MenuFocusEvidence {
            at_click: focus,
            before_dispatch: MenuFocusSnapshot {
                app_active: self.app.isActive(),
                key_window_present: self.app.keyWindow().is_some(),
                main_window_present: self.app.mainWindow().is_some(),
            },
        })
    }

    /// Consume a refused invocation without effects; leaves the menu installed.
    pub fn discard(&mut self, activation: MenuActivation) {
        let mut state = self.state.borrow_mut();
        if state
            .pending
            .as_ref()
            .is_some_and(|pending| pending.stamp == activation)
        {
            state.pending = None;
        }
    }

    /// Execute a one-shot native action only after Core rechecks its current
    /// authenticated role, menu generation and verified menu Guard grant.
    /// `request_close` is the existing UI registry close-request owner, never a
    /// native close/destroy operation. Its accepted request still awaits veto.
    ///
    /// # Errors
    /// Refuses stale/replayed invocations, changed focus or an unavailable close
    /// target. It never retargets an old activation to a newly focused window.
    pub fn dispatch(
        &mut self,
        activation: MenuActivation,
        request_close: &mut MenuCloseRequest<'_>,
    ) -> Result<MacMenuOutcome, IpcError> {
        if !self.owns_actual_menu() {
            self.cancel();
            return Err(invalid("native menu tree is no longer installed"));
        }
        let pending = {
            let mut state = self.state.borrow_mut();
            current_pending(&state, activation)?;
            state
                .pending
                .take()
                .ok_or_else(|| invalid("native menu invocation was consumed"))?
        };
        match pending.action {
            MenuAction::UnsupportedClick => Ok(MacMenuOutcome::UnsupportedClick),
            MenuAction::Inert => Err(invalid("inert native menu item cannot be activated")),
            MenuAction::NativeRole(MenuRole::Quit) => Ok(MacMenuOutcome::UnsupportedQuit),
            MenuAction::NativeRole(MenuRole::Hide) => {
                self.app.hide(None);
                Ok(MacMenuOutcome::Applied)
            }
            MenuAction::NativeRole(MenuRole::HideOthers) => {
                self.app.hideOtherApplications(None);
                Ok(MacMenuOutcome::Applied)
            }
            MenuAction::NativeRole(MenuRole::Unhide) => {
                self.app.unhideAllApplications(None);
                Ok(MacMenuOutcome::Applied)
            }
            MenuAction::NativeRole(role) => {
                let window = captured_window(&pending)?;
                if !self.app.keyWindow().is_some_and(|current| {
                    std::ptr::eq::<NSWindow>(current.as_ref(), window.as_ref())
                }) {
                    return Err(invalid(
                        "native menu focused window changed before admission",
                    ));
                }
                if role == MenuRole::Close {
                    request_close(&window)?;
                    return Ok(MacMenuOutcome::CloseRequested);
                }
                let responder = pending
                    .responder
                    .as_ref()
                    .and_then(Weak::load)
                    .ok_or_else(|| invalid("native menu action has no focused responder"))?;
                if !window.firstResponder().is_some_and(|current| {
                    std::ptr::eq::<NSResponder>(current.as_ref(), responder.as_ref())
                }) {
                    return Err(invalid(
                        "native menu focused responder changed before admission",
                    ));
                }
                Ok(if target::perform_edit(&responder, role) {
                    MacMenuOutcome::Applied
                } else {
                    MacMenuOutcome::UnsupportedResponder
                })
            }
        }
    }

    fn owns_actual_menu(&self) -> bool {
        self.installed.as_ref().is_some_and(|installed| {
            self.app.mainMenu().is_some_and(|actual| {
                std::ptr::eq::<NSMenu>(actual.as_ref(), installed.root.as_ref())
            })
        })
    }
}

impl Drop for MacApplicationMenu {
    fn drop(&mut self) {
        self.cancel();
        if self.owns_actual_menu() {
            self.app.setMainMenu(None);
        }
        // InstalledMenu clears weak target references before dropping targets.
        self.installed = None;
    }
}

/// No focus fallback: these are different errors so the current OS witness can
/// distinguish missing click-time authority from a retired weak native handle.
fn captured_window(pending: &PendingActivation) -> Result<Retained<NSWindow>, IpcError> {
    let captured = pending
        .window
        .as_ref()
        .ok_or_else(|| invalid("native menu action had no key window at click"))?;
    captured
        .load()
        .ok_or_else(|| invalid("native menu click-time key window was released"))
}

fn current_pending(
    state: &ActivationState,
    activation: MenuActivation,
) -> Result<&PendingActivation, IpcError> {
    if !state.active || activation.generation != state.generation {
        return Err(invalid("native menu activation has a retired generation"));
    }
    state
        .pending
        .as_ref()
        .filter(|pending| pending.stamp == activation)
        .ok_or_else(|| invalid("native menu activation is stale or already consumed"))
}

fn modifiers(role: MenuRole) -> NSEventModifierFlags {
    let source = role.modifiers();
    let mut modifiers = NSEventModifierFlags::empty();
    if source & 1 != 0 {
        modifiers |= NSEventModifierFlags::Command;
    }
    if source & 2 != 0 {
        modifiers |= NSEventModifierFlags::Shift;
    }
    if source & 4 != 0 {
        modifiers |= NSEventModifierFlags::Option;
    }
    modifiers
}

#[cfg(test)]
mod tests;
