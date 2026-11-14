//! Borrowed fixed trace records: no rendered JSON String or endpoint forwarding.
use std::io::Write as _;

use serde::Serialize;
use windows::Win32::Foundation::E_UNEXPECTED;

use crate::WebviewId;

fn emit<T: Serialize>(prefix: &str, value: &T) -> windows::core::Result<()> {
    let stderr = std::io::stderr();
    let mut output = stderr.lock();
    write!(output, "{prefix} ").map_err(|_| E_UNEXPECTED)?;
    serde_json::to_writer(&mut output, value).map_err(|_| E_UNEXPECTED)?;
    writeln!(output).map_err(|_| E_UNEXPECTED)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Fixed trace fields preserve independent actual identity, registration and destruction observations"
)]
pub(super) struct Ready {
    pub(super) kind: &'static str,
    pub(super) view: u32,
    pub(super) sender_matches_top: bool,
    pub(super) message_registration_returned: bool,
    pub(super) destroyed_registration_returned: bool,
    pub(super) already_destroyed: bool,
}

pub(super) fn ready(record: &Ready) -> windows::core::Result<()> {
    emit("KELD_X3_FRAME", record)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Message<'a> {
    kind: &'static str,
    view: u32,
    sender_matches_created_frame: bool,
    source: &'a str,
    outer_json: &'a str,
}

pub(super) fn message(
    id: WebviewId,
    same: bool,
    source: &str,
    outer_json: &str,
) -> windows::core::Result<()> {
    emit(
        "KELD_X3_FRAME",
        &Message {
            kind: "message",
            view: id.0,
            sender_matches_created_frame: same,
            source,
            outer_json,
        },
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Destroyed {
    kind: &'static str,
    view: u32,
    sender_matches_created_frame: bool,
}

pub(super) fn destroyed(id: WebviewId, same: bool) -> windows::core::Result<()> {
    emit(
        "KELD_X3_FRAME",
        &Destroyed {
            kind: "destroyed",
            view: id.0,
            sender_matches_created_frame: same,
        },
    )
}

#[derive(Serialize)]
struct Fault {
    view: u32,
    detail: &'static str,
}

pub(super) fn fault(id: WebviewId, detail: &'static str) -> windows::core::Result<()> {
    emit("KELD_X3_FRAME_FAULT", &Fault { view: id.0, detail })
}
