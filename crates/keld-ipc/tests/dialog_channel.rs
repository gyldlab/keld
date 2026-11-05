//! GH-478: dialog is an append-only guarded channel, not an authority itself.

use keld_guard::capability::DIALOG;
use keld_ipc::channel_table::{Authority, CHANNEL_TABLE, ReceiveClass, WINDOW};

#[test]
fn dialog_is_the_next_guarded_channel() {
    let window_index = CHANNEL_TABLE
        .iter()
        .position(|entry| entry.id() == WINDOW.id())
        .expect("window channel remains allocated");
    let dialog = CHANNEL_TABLE
        .get(window_index + 1)
        .expect("dialog remains immediately after window in allocation order");
    assert_eq!(dialog.name(), "dialog");
    assert_eq!(dialog.wire_id(), WINDOW.wire_id() + 1);
    assert_eq!(dialog.class(), ReceiveClass::GuardedCall);
    assert_eq!(dialog.authority(), Authority::Guarded(&[DIALOG]));
}
