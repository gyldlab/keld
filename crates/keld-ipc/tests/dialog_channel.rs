//! GH-478: dialog is an append-only guarded channel, not an authority itself.

use keld_guard::capability::DIALOG;
use keld_ipc::channel_table::{Authority, DIALOG as DIALOG_CHANNEL, ReceiveClass, WINDOW, entry};

#[test]
fn dialog_keeps_its_guarded_allocation() {
    let dialog = entry(DIALOG_CHANNEL.id()).expect("dialog in channel table");
    assert_eq!(dialog.name(), "dialog");
    assert_eq!(dialog.wire_id(), WINDOW.wire_id() + 1);
    assert_eq!(dialog.class(), ReceiveClass::GuardedCall);
    assert_eq!(dialog.authority(), Authority::Guarded(&[DIALOG]));
}
