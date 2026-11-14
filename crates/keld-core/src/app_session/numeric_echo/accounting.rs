//! Existing numeric `LocalCallBudget` storage covers the Core read overlap too.
//! No CONTROL reservation, additional quota, or result allocation is minted.
use super::{
    Call, CallError, HostAppError, NativeNumericResultReservation, PendingPrimaryEcho,
    PrimaryEchoReply, RendererBridgeOutcome, app_detail, read,
};
use std::mem::size_of;

/// Conservative backing bound for the installed Rust 1.99.1 bounded mpmc
/// channel: `Counter<array::Channel<_>>`, its two 128-byte padded indices, two
/// Mutex<Waker> values, the one Slot stamp, and at most four receiver Waker
/// entries. There is one receiver and only `try_send`, so sender waiters never
/// allocate and the receiver Vec cannot grow beyond its first four entries.
/// The actual message/record/guard sizes are added separately below. Existing
/// worker/TLS lifetime is unchanged; this bound covers per-call channel backing.
/// Pinned source hashes and the field census are in numeric-channel-layout.json.
const STD_CHANNEL_BACKING_BYTES: usize = 4096;

pub(super) fn metadata_bytes() -> usize {
    STD_CHANNEL_BACKING_BYTES
        + size_of::<Result<PrimaryEchoReply, HostAppError>>()
        + size_of::<PendingPrimaryEcho>()
        + size_of::<Call>()
        + size_of::<read::ReadGuard<'static>>()
        + size_of::<keld_ipc::ValidatedFrameHeader>()
        + size_of::<CallError>()
        + size_of::<RendererBridgeOutcome>()
}

/// Called before `sync_channel` and before the canonical reader may allocate.
/// The canonical <=4096 ERR body and its decoded two Strings may coexist.
/// Their actual capacities are checked again before the raw body is dropped.
pub(in crate::app_session) fn require_prepaid(
    storage: &NativeNumericResultReservation,
) -> Result<(), HostAppError> {
    let maximum = keld_wv::wv_link::MAX_RENDERER_PAYLOAD_LEN;
    if storage.maximum_payload_len() != maximum || !covers(storage, maximum.saturating_mul(2)) {
        return Err(app_detail(
            "numeric Echo storage",
            "original result allowance is insufficient",
        ));
    }
    Ok(())
}

pub(in crate::app_session) fn covers(
    storage: &NativeNumericResultReservation,
    buffers: usize,
) -> bool {
    buffers
        .checked_add(metadata_bytes())
        .is_some_and(|total| storage.covers_storage(total))
}
