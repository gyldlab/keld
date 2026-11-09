//! GH-478: dialog is an append-only guarded channel, not an authority itself.

use keld_guard::capability::DIALOG;
use keld_ipc::channel_table::{Authority, CHANNEL_TABLE, ReceiveClass, WINDOW};

#[test]
fn dialog_is_the_next_guarded_channel() {
    let dialog = CHANNEL_TABLE.last().expect("channel table is nonempty");
    assert_eq!(dialog.name(), "dialog");
    assert_eq!(dialog.wire_id(), WINDOW.wire_id() + 1);
    assert_eq!(dialog.class(), ReceiveClass::GuardedCall);
    assert_eq!(dialog.authority(), Authority::Guarded(&[DIALOG]));
}
